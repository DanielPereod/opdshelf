package main

import (
	"fmt"
	"html/template"
	"io"
	"log"
	"mime"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/gorilla/mux"
)

const (
	PORT      = "3000"
	HOST      = "0.0.0.0"
	BOOKS_DIR = "./books"
)

// BookInfo represents metadata about a book
type BookInfo struct {
	Filename    string
	Title       string
	MimeType    string
	LastUpdated time.Time
	Size        int64
}

// TemplateData holds data passed to HTML templates
type TemplateData struct {
	Books       []BookInfo
	BaseURL     string
	CurrentTime string
}

// Template functions to improve display
var templateFuncs = template.FuncMap{
	"formatDate": formatDate,
	"formatSize": formatSize,
	"even": func(i int) bool {
		return i%2 == 0
	},
}

// formatDate formats a time.Time into a readable date string
func formatDate(t time.Time) string {
	return t.Format("Jan 02, 2006 15:04")
}

// formatSize converts file size in bytes to a readable format
func formatSize(size int64) string {
	const unit = 1024
	if size < unit {
		return fmt.Sprintf("%d B", size)
	}
	div, exp := int64(unit), 0
	for n := size / unit; n >= unit; n /= unit {
		div *= unit
		exp++
	}
	return fmt.Sprintf("%.1f %cB", float64(size)/float64(div), "KMGTPE"[exp])
}

func main() {
	// Ensure books directory exists
	if err := os.MkdirAll(BOOKS_DIR, 0755); err != nil {
		log.Fatalf("Failed to create books directory: %v", err)
	}

	// Register mime types if needed
	mime.AddExtensionType(".epub", "application/epub+zip")
	mime.AddExtensionType(".fb2", "application/x-fictionbook+xml")
	mime.AddExtensionType(".fb2.zip", "application/zip")

	// Register template functions
	templates := template.New("")
	templates.Funcs(template.FuncMap{
		"formatSize": formatSize,
		"formatDate": formatDate,
	})

	// Create router
	r := mux.NewRouter()

	// Middleware for logging requests
	r.Use(loggingMiddleware)

	// Routes
	r.HandleFunc("/", opdsIndexHandler).Methods("GET")
	r.HandleFunc("/admin", adminHandler).Methods("GET")
	r.HandleFunc("/upload", uploadHandler).Methods("POST")
	r.HandleFunc("/delete/{filename}", deleteHandler).Methods("POST")

	// Serve books directory
	r.PathPrefix("/books/").Handler(http.StripPrefix("/books/", http.FileServer(http.Dir(BOOKS_DIR))))

	// Serve static files
	r.PathPrefix("/static/").Handler(http.StripPrefix("/static/", http.FileServer(http.Dir("./static"))))

	// Start server
	serverAddr := HOST + ":" + PORT
	fmt.Printf("OPDS Server running at http://%s\n", serverAddr)
	log.Fatal(http.ListenAndServe(serverAddr, r))
}

// Middleware for logging requests
func loggingMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		log.Printf("%s %s", r.Method, r.URL.Path)
		next.ServeHTTP(w, r)
	})
}

// Handler for OPDS catalog
func opdsIndexHandler(w http.ResponseWriter, r *http.Request) {
	books, err := getBooksList()
	if err != nil {
		http.Error(w, "Failed to read books directory", http.StatusInternalServerError)
		return
	}

	data := TemplateData{
		Books:       books,
		BaseURL:     getBaseURL(r),
		CurrentTime: time.Now().UTC().Format(time.RFC3339),
	}

	// Set content type for OPDS
	w.Header().Set("Content-Type", "application/atom+xml;charset=utf-8;profile=opds-catalog;kind=acquisition")

	// Write XML header directly to avoid template escaping
	fmt.Fprint(w, `<?xml version="1.0" encoding="UTF-8"?>`)

	// Use template for rest of OPDS XML
	tmpl, err := template.New("opds.xml").Funcs(templateFuncs).ParseFiles("templates/opds.xml")
	if err != nil {
		log.Printf("Error parsing template: %v", err)
		http.Error(w, "Error generating OPDS feed", http.StatusInternalServerError)
		return
	}

	if err := tmpl.Execute(w, data); err != nil {
		log.Printf("Error executing template: %v", err)
		http.Error(w, "Error generating OPDS feed", http.StatusInternalServerError)
	}
}

// Handler for admin page
func adminHandler(w http.ResponseWriter, r *http.Request) {
	books, err := getBooksList()
	if err != nil {
		http.Error(w, "Failed to read books directory", http.StatusInternalServerError)
		return
	}

	data := TemplateData{
		Books:   books,
		BaseURL: getBaseURL(r),
	}

	// Parse templates with function map
	tmpl, err := template.New("layout.html").Funcs(templateFuncs).ParseFiles(
		"templates/layout.html",
		"templates/admin.html",
	)
	if err != nil {
		log.Printf("Error parsing template: %v", err)
		http.Error(w, "Error rendering admin page", http.StatusInternalServerError)
		return
	}

	if err := tmpl.ExecuteTemplate(w, "layout.html", data); err != nil {
		log.Printf("Error executing template: %v", err)
		http.Error(w, "Error rendering admin page", http.StatusInternalServerError)
	}
}

// Handler for file uploads
func uploadHandler(w http.ResponseWriter, r *http.Request) {
	// Parse multipart form with 32MB max memory
	if err := r.ParseMultipartForm(32 << 20); err != nil {
		http.Error(w, "Error parsing form", http.StatusBadRequest)
		return
	}

	// Get file from form
	file, header, err := r.FormFile("book")
	if err != nil {
		http.Error(w, "Error retrieving file", http.StatusBadRequest)
		return
	}
	defer file.Close()

	// Create destination file
	filePath := filepath.Join(BOOKS_DIR, header.Filename)
	dest, err := os.Create(filePath)
	if err != nil {
		http.Error(w, "Error saving file", http.StatusInternalServerError)
		return
	}
	defer dest.Close()

	// Copy file content
	if _, err := io.Copy(dest, file); err != nil {
		http.Error(w, "Error saving file", http.StatusInternalServerError)
		return
	}

	// Redirect back to admin page
	http.Redirect(w, r, "/admin", http.StatusSeeOther)
}

// Handler for file deletion
func deleteHandler(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	filename := vars["filename"]

	// Sanitize filename for safety
	filePath := filepath.Join(BOOKS_DIR, filepath.Clean(filename))

	// Check if file exists
	if _, err := os.Stat(filePath); os.IsNotExist(err) {
		http.Error(w, "File not found", http.StatusNotFound)
		return
	}

	// Delete file
	if err := os.Remove(filePath); err != nil {
		log.Printf("Error deleting file: %v", err)
		http.Error(w, "Error deleting file", http.StatusInternalServerError)
		return
	}

	// Redirect back to admin page
	http.Redirect(w, r, "/admin", http.StatusSeeOther)
}

// Add this function before getBooksList()
func cleanupTitle(filename string) string {
	// Remove file extension
	title := strings.TrimSuffix(filename, filepath.Ext(filename))
	// Replace common separators with spaces
	title = strings.NewReplacer("-", " ", "_", " ").Replace(title)
	return strings.TrimSpace(title)
}

// Helper function to get list of books
func getBooksList() ([]BookInfo, error) {
	var books []BookInfo

	files, err := os.ReadDir(BOOKS_DIR)
	if err != nil {
		return nil, err
	}

	for _, file := range files {
		if file.IsDir() {
			continue
		}

		filePath := filepath.Join(BOOKS_DIR, file.Name())
		fileInfo, err := os.Stat(filePath)
		if err != nil {
			log.Printf("Error stating file %s: %v", file.Name(), err)
			continue
		}

		mimeType := mime.TypeByExtension(filepath.Ext(file.Name()))
		if mimeType == "" {
			mimeType = "application/octet-stream"
		}

		books = append(books, BookInfo{
			Filename:    file.Name(),
			Title:       cleanupTitle(file.Name()),
			MimeType:    mimeType,
			LastUpdated: fileInfo.ModTime().UTC(),
			Size:        fileInfo.Size(),
		})
	}

	return books, nil
}

// Helper function to get base URL
func getBaseURL(r *http.Request) string {
	scheme := "http"
	if r.TLS != nil {
		scheme = "https"
	}
	return fmt.Sprintf("%s://%s", scheme, r.Host)
}
