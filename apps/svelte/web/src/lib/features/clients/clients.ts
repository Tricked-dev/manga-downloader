export interface ClientDownload {
  href: string;
  labelKey: string;
  filename: string;
  descriptionKey: string;
}

export interface ClientPackage {
  id: string;
  name: string;
  platform: string;
  descriptionKey: string;
  downloads: ClientDownload[];
}

export const clientPackages: ClientPackage[] = [
  {
    id: "aidoku",
    name: "Aidoku",
    platform: "iOS / iPadOS",
    descriptionKey: "app.clients.packageData.aidoku.description",
    downloads: [
      {
        href: "/api/app/clients/aidoku/package",
        labelKey: "app.clients.packageData.aidoku.downloadLabel",
        filename: "package.aix",
        descriptionKey: "app.clients.packageData.aidoku.downloadDescription",
      },
    ],
  },
  {
    id: "tachiyomi",
    name: "Tachiyomi / Mihon",
    platform: "Android",
    descriptionKey: "app.clients.packageData.tachiyomi.description",
    downloads: [
      {
        href: "/api/app/clients/tachiyomi/package",
        labelKey: "app.clients.packageData.tachiyomi.downloadLabel",
        filename: "manga-downloader-tachiyomi.apk",
        descriptionKey: "app.clients.packageData.tachiyomi.downloadDescription",
      },
    ],
  },
];
