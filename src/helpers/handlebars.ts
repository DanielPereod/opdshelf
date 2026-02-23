import { formatSize, getSimpleMime } from "../utils";
import Handlebars from "handlebars";

export function registerHandlebarsHelpers() {
  Handlebars.registerHelper('formatSize', formatSize);
  Handlebars.registerHelper('simpleMime', getSimpleMime);
  Handlebars.registerHelper('eq', (a, b) => a === b);
  Handlebars.registerHelper('or', (a, b) => a || b);
  Handlebars.registerHelper('len', (arr: any[]) => arr?.length || 0);
  Handlebars.registerHelper('urlEncode', (str: string) => str.split('/').map(encodeURIComponent).join('/'));
}
