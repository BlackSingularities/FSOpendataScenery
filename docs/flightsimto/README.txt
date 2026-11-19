FS Opendata Scenery 1.0.0
Terrain, street lights and obstacles from open data for MSFS 2024
Design and development: redzikowski.com

==============================================================================
 WHAT'S INSIDE
==============================================================================

  FSOpendataScenery_1.0.0_x64-setup.exe   Windows program (installer)
  README.txt                              this file

FS Opendata Scenery is a standalone Windows program that MAKES scenery
packages for the Community folder. Do not put this ZIP or the program
in your Community folder.

==============================================================================
 1. INSTALL
==============================================================================

1. Run FSOpendataScenery_1.0.0_x64-setup.exe and follow the installer.
   If Windows SmartScreen warns about the file: More info -> Run anyway.
2. Start FS Opendata Scenery. Your MSFS 2024 installation, the Community
   folder and the MSFS 2024 SDK are detected automatically. You can change
   them in Settings.

==============================================================================
 2. MSFS 2024 SDK (needed to compile packages)
==============================================================================

The SDK is free. In MSFS 2024 turn on Developer Mode, then open
Help -> SDK Installer and install it. Restart FS Opendata Scenery
afterwards.

Without the SDK the app still writes the complete project sources, which
you can build later in Developer Mode.

Close the simulator while a package is being built: fspackagetool starts
its own copy of MSFS in the background.

==============================================================================
 3. FIRST SCENERY IN FIVE MINUTES
==============================================================================

1. Home -> New project. Draw a rectangle or polygon on the map, search for
   a town, or take a circle around an airport.
2. Terrain -> Compute terrain. In Poland the 1 m LiDAR model from GUGiK is
   used; elsewhere AWS Terrain Tiles / Copernicus, or your own GeoTIFF,
   ASC, XYZ or HGT files.
3. Lights -> Place lights. Real street lamps and lit roads come from
   OpenStreetMap.
4. Objects -> obstruction lights, lighthouses and tree removal (optional).
5. 3D preview -> check everything by night and by day.
6. Build -> Build and install. Start MSFS and fly to your area at night.

Tip: before a big area, use "Build and install test" in the Build view.
It places three lamps of every category, an obstruction light and a
lighthouse in the middle of the area. If the lamps look too bright or too
dark, use Brighter / Dimmer in the Lights view.

==============================================================================
 4. UPDATES - YOU ONLY DOWNLOAD ONCE
==============================================================================

The program checks for a new version at startup ("Check for updates" in
Settings) and installs it in place. Your projects and settings stay.

==============================================================================
 5. HELP AND FEEDBACK
==============================================================================

- In the app: Settings -> Report a problem or suggest a feature
- Website: https://redzikowski.com/portfolio/fsopendatascenery
- Source code (MIT): https://github.com/BlackSingularities/FSOpendataScenery

Map data (c) OpenStreetMap contributors (ODbL). Elevation data: GUGiK,
Copernicus DEM (c) DLR e.V., (c) Airbus Defence and Space, ESA,
AWS Terrain Tiles (Mapzen / AWS Open Data). Airports: OurAirports.
Microsoft Flight Simulator is a trademark of Microsoft Corporation.
FS Opendata Scenery is an independent project and is not affiliated with
Microsoft or Asobo Studio.
