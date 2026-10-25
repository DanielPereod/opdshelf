import AdmZip from 'adm-zip';
import path from 'path';
import mime from 'mime-types';
import fs from 'node:fs';
import { unzipSync } from "fflate";
import { DOMParser } from "linkedom";
import { BookInfo } from '../types';

const SUPPORTED_ARCHIVES = ['.epub', '.cbz', '.zip', '.fb2.zip'];
const COVER_REGEX = /(^|\/)cover\.(jpe?g|png)$/i;
const IMAGE_REGEX = /\.(jpe?g|png)$/i;

export async function getEpubInfo(filePath: string): Promise<BookInfo | null> {
  try {

    const file = unzipSync(new Uint8Array(await Bun.file(filePath).arrayBuffer()));
    const rootOPF = file["META-INF/container.xml"];
    const documentOPF = new DOMParser().parseFromString(new TextDecoder().decode(rootOPF), "text/xml");
    const contentOPF = documentOPF.querySelector("rootfile")?.getAttribute("full-path")!;
    const route = contentOPF.split("/").slice(0, -1).join("/");

    const document = new DOMParser().parseFromString(new TextDecoder().decode(file[contentOPF]), "text/xml");
    const coverID = document.querySelector("meta[name='cover']")?.getAttribute("content");
    const coverHref = coverID ? document.querySelector(`item[id='${coverID}']`)?.getAttribute("href") : null;
    const coverImage = coverHref ? file[path.posix.join(route, coverHref)] : undefined;

    const bookInfo: BookInfo = {
      title: document.querySelector("dc\\:title")?.textContent,
      creator: document.querySelector("dc\\:creator")?.textContent,
      identifier: document.querySelector("dc\\:identifier")?.textContent,
      language: document.querySelector("dc\\:language")?.textContent,
      publisher: document.querySelector("dc\\:publisher")?.textContent,
      subject: document.querySelector("dc\\:subject")?.textContent,
      description: document.querySelector("dc\\:description")?.textContent || document.querySelector("description")?.textContent,
      date: document.querySelector("dc\\:date")?.textContent,
      cover: coverImage
    }

    return bookInfo
  } catch (e) {
    return null;
  }
}

export const getBookInfo = async (filePath: string): Promise<BookInfo | null> => {
  try {
    const ext = path.extname(filePath).toLowerCase();

    if (ext === '.epub') {
      const epubInfo = await getEpubInfo(filePath);
      if (epubInfo) return epubInfo;
    }

    if (SUPPORTED_ARCHIVES.includes(ext)) {
      const buffer = await fs.promises.readFile(filePath);
      const zip = new AdmZip(buffer);
      const entries = zip.getEntries();

      let entry = entries.find(e => COVER_REGEX.test(e.entryName) && !e.isDirectory);

      if (!entry) {
        const images = entries.filter(e =>
          IMAGE_REGEX.test(e.entryName) &&
          !e.entryName.includes('__MACOSX') &&
          !e.isDirectory
        );

        if (images.length > 0) {
          images.sort((a, b) => {
            const sizeA = (a as any).header?.size || 0;
            const sizeB = (b as any).header?.size || 0;
            return sizeB - sizeA;
          });
          entry = images[0];
        }
      }

      if (entry) {
        return {
          cover: entry.getData(),
        };
      }
    }

    return null;
  } catch (err) {
    console.error(err);
    return null;
  }
};
