# flightsim.to – listing

Everything below is in English, ready to paste into the flightsim.to form
(Manage → General / Versioning / Images / FAQ).

- Screenshots: `docs/flightsimto/screenshots/` (1920×1080, captions at the bottom)
- Poster / cover image: `docs/flightsimto/poster/FSOpendataScenery-poster-1920x1080.png` (4K version next to it)
- Download ZIP: `FSOpendataScenery-1.0.0.zip` = installer from the GitHub release + `README.txt` from this folder
- Regenerating the images: `docs/flightsimto/generator/` (see the comment at the top of each script)

---

## Title

FS Opendata Scenery – terrain, street lights and obstacles from open data for MSFS 2024

## Short description

Make your own scenery add-ons from free open data. Draw an area on the map and the app builds more accurate terrain (1 m LiDAR in Poland, free DEMs worldwide or your own files), places real street lamps from OpenStreetMap with light pointing down, adds obstruction lights and lighthouses, clears trees from roads and water, and compiles a ready Community package. Free, open source, updates itself.

## Compatibility

Microsoft Flight Simulator 2024

## Category

Utilities / Tools (scenery tool)

## Keywords (Search Keywords)

scenery generator, terrain, mesh, heightmap, lidar, dem, elevation, street lights, night lighting, openstreetmap, osm, obstruction lights, lighthouse, tree removal, exclusion, community folder, sdk, fspackagetool, poland, utility, tool, free, open source, msfs 2024

---

## Description

**Your area in MSFS 2024: more accurate, and lit like the real thing.**

FS Opendata Scenery is a free, open-source Windows app that **creates scenery add-ons for the Community folder** from open data. You choose an area on the map, the app downloads free elevation models and OpenStreetMap data, computes better terrain, places lights where they really are, and compiles everything into a package with the official MSFS 2024 SDK tool. No 3D modelling, no XML editing and no GIS software needed.

### How it works

1. **Area**: draw a rectangle, polygon or circle, take a circle around an airport (ICAO), use the outline of a town from the search box, or import KML, GeoJSON, GPX or WKT.
2. **Terrain**: pick and order your elevation sources and press *Compute terrain*.
3. **Lights and objects**: fetch OpenStreetMap data and press *Place lights*.
4. **Build**: preview everything in 3D by night and by day, then build and install the package with one click.

### 🏔️ Terrain

- **Sources in the order you choose.** At every point the first source with data wins:
  - **GUGiK DTM 1 m**: the whole of Poland, LiDAR, streamed straight from the Geoportal
  - **AWS Terrain Tiles**: worldwide (SRTM, EU-DEM, USGS 3DEP, LiDAR for the UK, Austria, Norway, New Zealand and more)
  - **Copernicus GLO-30 / GLO-90**
  - **Your own files**: GeoTIFF, ESRI ASCII Grid, XYZ, SRTM HGT, also zipped (for example LiDAR from swisstopo, AHN, data.gov.uk or other national portals)
  - **Your own WCS 1.0 service**
- **More than 20 coordinate systems built in**, no GDAL required: WGS 84, Web Mercator, UTM / ETRS89-UTM, Polish PL-1992 and PL-2000, Swiss LV95/LV03, Dutch RD, British National Grid, SWEREF99, ETRS-TM35FIN, D96/TM, NZTM and more. The system is read from GeoTIFF and `.prj` files.
- MSFS height-map rectangles with no seams: grid step from 1 to 30 m, a smooth transition (falloff) to the default terrain, and priority.
- **Runways from OSM are flattened**: the length profile follows the real runway, the cross-section is level, and the edges and thresholds blend in smoothly. No more LiDAR bumps under your wheels.
- Lakes and ponds from OSM are levelled to the water surface, flat tiles and sea are skipped automatically, and you can smooth the result without seams or shift it vertically.
- Shaded preview on the map, an elevation profile tool, statistics and the share of each source.
- Export the computed terrain to **GeoTIFF** (WGS 84, float32) for QGIS or other tools.

### 💡 Street lights

- **Real street lamps from OpenStreetMap** (`highway=street_lamp`). Height, mount (arm, post-top, wall, suspended, floodlight) and colour (sodium, LED, mercury) come from the tags, and the arm is turned over the nearest road.
- **Lit roads** (`lit=yes`) get lamps with spacing, height and side that depend on the road class, with optional lighting for unlit-but-urban streets. Lamps never duplicate the mapped ones and never stand on a carriageway, and sidewalks along a lit street share its lamps instead of getting a second row of poles.
- **Car parks** (a grid of masts), **sports pitches and stadiums** (corner masts with floodlights tilted onto the pitch), **railway platforms**, **fuel stations** and **piers**.
- The light **points downwards from the real height**, and every lamp **stands on the ground**, including on your new terrain.
- Every category has its own height, spacing, colour temperature, intensity, cone, look and arm length. Quick settings give every category one colour or make all lights brighter or dimmer at once.
- Place lamps by hand, exclude single mapped lamps, and export everything to GeoJSON.
- **Lighting test package**: three lamps of every category, an obstruction light and a lighthouse in the middle of your area, in one tiny package. Fly there at night and check brightness and direction in a minute before you build a whole city.

### 🗼 Obstacles and trees

- **Obstruction lights** on masts, chimneys, towers, wind turbines (at hub height), cranes and pylons: steady or flashing, with intermediate levels on tall structures.
- **Lighthouses** with their real colour, period and flash pattern from the `seamark:*` tags.
- An optional simple wind turbine model where the simulator shows none.
- **Tree removal**: exclusion polygons clear autogen trees from roads, railways, water, runways, pitches and car parks.

### 📦 Package

- Procedural glTF models with `ASOBO_macro_light` pointing down; fixtures glow only at night and come with two levels of detail.
- Objects use `altitudeIsAgl`, so they follow the terrain, including your new terrain.
- A complete MSFS 2024 SDK project (package definition, scenery XML, model library and thumbnail), compiled with **`fspackagetool.exe`** with a live log.
- **Install into Community** by copying or as a folder link, uninstall, create a ZIP to share, and export or import whole projects.
- **3D preview** of terrain and lights by night and by day, with height exaggeration.
- The dashboard lists the packages installed in Community (your own and test ones) and can remove them.

### Quality of life

- English and Polish interface; dark, light and AMOLED themes with accent colours
- A short guide on first start and undo for area edits (Ctrl+Z)
- A cache of downloaded data, with its size shown and a clear button
- Built-in updates (installer and portable version) and a *Report a problem or suggest a feature* form

### Requirements

- Windows 10 or 11 (64-bit) with WebView2, which is built into Windows 11
- **Microsoft Flight Simulator 2024**
- **MSFS 2024 SDK** to compile packages. It is free: in the simulator turn on *Developer Mode*, then *Help → SDK Installer*. Without the SDK the app still writes the complete project sources, which you can build later in Developer Mode.
- MSFS must be **closed while a package is being built**, because `fspackagetool` starts its own copy of the simulator in the background.

### Installation

1. Download the ZIP and run `FSOpendataScenery_1.0.0_x64-setup.exe`. If Windows SmartScreen warns about the file, choose *More info → Run anyway*. You can also use the portable `.exe` from GitHub without installing.
2. Start **FS Opendata Scenery**. Your MSFS 2024 installation, Community folder and SDK are detected automatically. You can change them in **Settings**.
3. Create a project, choose an area, compute terrain and place lights, then press **Build and install**.
4. Start the simulator and fly to your area.

Do **not** put this app in your Community folder. It is a standalone Windows program; only the packages it builds go there.

### FAQ

**Is this a scenery package?**
No. It is a tool that *makes* scenery packages for any area you choose. The packages are yours to use or share.

**Where does the terrain data come from, and how good is it?**
In Poland the app uses the 1 m LiDAR terrain model from GUGiK. Elsewhere it uses AWS Terrain Tiles (from about 1 m to 30 m, depending on the country) and Copernicus 30 m, or your own LiDAR files, which are often available for free from national geoportals.

**Do I need the SDK?**
Only to compile the final package. The free MSFS 2024 SDK is installed from Developer Mode in the simulator. Without it you get the complete project sources.

**My lamps look too bright or too dark.**
Build the lighting test package first, look at it in the sim at night, then use *Brighter* / *Dimmer* in the Lights view. If the arms point away from the road, turn on *Flip lamp arms*.

**Will it conflict with other scenery?**
Terrain rectangles have a priority setting, and you can skip airports that already have a custom add-on by leaving them out of the area. Lights and trees only affect the area you chose.

**Does it change the simulator or the files of other add-ons?**
No. It only writes its own package into Community, and only when you choose to install it.

### Feedback

Found a bug or have an idea? Use **Report a problem or suggest a feature** in the app's Settings, leave a comment here or open an issue on GitHub. Every report is read.

- Website: https://redzikowski.com/portfolio/fsopendatascenery
- Source code (MIT): https://github.com/BlackSingularities/FSOpendataScenery

*Map data © OpenStreetMap contributors (ODbL). Elevation data: GUGiK, Copernicus DEM (© DLR e.V., © Airbus Defence and Space, ESA), AWS Terrain Tiles (Mapzen / AWS Open Data). Airports: OurAirports (public domain). Microsoft Flight Simulator is a trademark of Microsoft Corporation. FS Opendata Scenery is an independent project and is not affiliated with Microsoft or Asobo Studio.*

---

## FAQ (Add FAQ)

**Q: Is this a scenery package?**
A: No, it is a Windows tool that makes scenery packages for any area you choose.

**Q: Which simulator does it support?**
A: Microsoft Flight Simulator 2024. Packages are compiled with the MSFS 2024 SDK.

**Q: Do I need the SDK?**
A: Only to compile the final package. It is free: Developer Mode → Help → SDK Installer.

**Q: Where does the terrain come from?**
A: 1 m LiDAR from GUGiK in Poland, AWS Terrain Tiles and Copernicus worldwide, or your own GeoTIFF/ASC/XYZ/HGT files.

**Q: My lamps are too bright or too dark.**
A: Build the lighting test package, check it at night, then use Brighter / Dimmer in the Lights view.

**Q: Do I have to come back here for every update?**
A: No. The app checks for new versions itself and updates in place.

---

## Version notes – 1.0.0

First release.

- Projects with areas drawn on the map, place and airport search, KML/GeoJSON/GPX/WKT import and undo.
- Terrain from GUGiK DTM 1 m, AWS Terrain Tiles, Copernicus GLO-30/90, your own files and WCS services, as MSFS height-map rectangles, with runway flattening, lake levelling and GeoTIFF export.
- Lighting from OpenStreetMap: street lamps, roads, paths, car parks, pitches, platforms, fuel stations and piers, plus manual lamps and a lighting test package.
- Obstruction lights, lighthouses and tree removal.
- Build with the MSFS 2024 SDK, install into Community, ZIP export, 3D preview.
- Built-in updates and feedback, English and Polish interface.

---

## Screenshot captions

| File | Caption |
| --- | --- |
| `01-lights-night.png` | Kraków Old Town: 3,143 lights, including 1,751 real street lamps from OpenStreetMap |
| `07-lights-satellite.png` | Real lamp positions over the Main Market Square, on satellite imagery |
| `02-terrain-lidar.png` | 1 m LiDAR terrain: Wawel Hill, the Vistula embankments and the old city walls |
| `03-3d-night.png` | 3D preview by night: terrain and every lamp before you build |
| `04-3d-day.png` | 3D preview by day with height exaggeration |
| `05-dashboard.png` | Dashboard: projects, simulator, Community folder and SDK at a glance |
| `06-area.png` | Draw the area, search for a town or airport, or import KML/GeoJSON/GPX |
| `09-terrain-light.png` | Elevation sources in the order you choose, here in the light theme |
| `10-objects.png` | Obstruction lights, lighthouses and tree removal |
| `08-build.png` | Build: readiness checklist, SDK compilation and install into Community |
| `11-settings.png` | Settings: simulator, SDK, cache, themes and language |

*The Kraków project uses real data (OpenStreetMap and GUGiK). The other project names on the dashboard are examples.*
