# Zmiany

## 1.0.1

- Rozpoznawanie SDK do MSFS 2020 (wersje 0.x, np. `C:\MSFS SDK`): aplikacja nie próbuje już nim kompilować (jego fspackagetool uruchamiał MSFS 2020 przez Steam i kończył się kodem 1), tylko pokazuje, że trzeba zainstalować MSFS 2024 SDK. Gdy na dysku są oba, wybiera SDK 2024.
- Gdy fspackagetool zakończy się błędem bez żadnego komunikatu, dziennik podpowiada, co sprawdzić.

## 1.0.0

Pierwsze wydanie.

- Projekty z obszarem rysowanym na mapie, wyszukiwarką miejsc i lotnisk oraz importem KML/GeoJSON/GPX/WKT.
- Rzeźba terenu z NMT GUGiK 1 m, AWS Terrain Tiles, Copernicus GLO-30/90, własnych plików i usług WCS – jako prostokąty z heightmapą MSFS.
- Oświetlenie z OpenStreetMap: latarnie, drogi, ścieżki, parkingi, boiska, perony, stacje paliw, pomosty; lampy ręczne.
- Światła przeszkodowe, latarnie morskie, wycinka roślinności.
- Kompilacja przez MSFS 2024 SDK, instalacja w Community, ZIP, podgląd 3D.
- Wyrównywanie pasów startowych z OSM w nowej rzeźbie terenu.
- Pakiet testowy do kalibracji jasności i kierunku świateł w symulatorze.
- Eksport terenu do GeoTIFF, lista pakietów w Community, przewodnik startowy, cofanie zmian obszaru.
- Aktualizacje i zgłoszenia przez redzikowski.com, polski i angielski interfejs.
