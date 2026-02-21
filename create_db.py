import os
import json
import sqlite3
import glob
from shapely.geometry import shape, MultiPolygon, Polygon
from shapely.wkb import dumps as wkb_dumps

# Konfigurasi
DB_FILE = "indonesia_area.db"
# Threshold simplifikasi (dalam derajat)
# 20 meter = ~0.00018 derajat (dibulatkan 0.0002)
SIMPLIFY_TOLERANCE = 0.0002
DATA_DIR = "./data"

def init_db(conn):
    cursor = conn.cursor()
    
    # Buat tabel provinces
    cursor.execute('''
    CREATE TABLE IF NOT EXISTS provinces (
        id TEXT PRIMARY KEY,
        name TEXT,
        lat REAL,
        lng REAL,
        min_lat REAL,
        max_lat REAL,
        min_lng REAL,
        max_lng REAL,
        boundaries BLOB
    )
    ''')
    
    # Buat tabel regencies
    cursor.execute('''
    CREATE TABLE IF NOT EXISTS regencies (
        id TEXT PRIMARY KEY,
        name TEXT,
        parent_id TEXT,
        lat REAL,
        lng REAL,
        min_lat REAL,
        max_lat REAL,
        min_lng REAL,
        max_lng REAL,
        boundaries BLOB,
        FOREIGN KEY (parent_id) REFERENCES provinces(id)
    )
    ''')
    
    # Buat tabel districts
    cursor.execute('''
    CREATE TABLE IF NOT EXISTS districts (
        id TEXT PRIMARY KEY,
        name TEXT,
        parent_id TEXT,
        lat REAL,
        lng REAL,
        min_lat REAL,
        max_lat REAL,
        min_lng REAL,
        max_lng REAL,
        boundaries BLOB,
        FOREIGN KEY (parent_id) REFERENCES regencies(id)
    )
    ''')
    
    # Buat tabel villages
    cursor.execute('''
    CREATE TABLE IF NOT EXISTS villages (
        id TEXT PRIMARY KEY,
        name TEXT,
        parent_id TEXT,
        lat REAL,
        lng REAL,
        min_lat REAL,
        max_lat REAL,
        min_lng REAL,
        max_lng REAL,
        boundaries BLOB,
        FOREIGN KEY (parent_id) REFERENCES districts(id)
    )
    ''')
    
    # Create indexes for spatial queries
    cursor.execute('CREATE INDEX IF NOT EXISTS idx_villages_lat_lng ON villages(lat, lng)')
    cursor.execute('CREATE INDEX IF NOT EXISTS idx_villages_bbox ON villages(min_lat, max_lat, min_lng, max_lng)')
    cursor.execute('CREATE INDEX IF NOT EXISTS idx_districts_lat_lng ON districts(lat, lng)')
    cursor.execute('CREATE INDEX IF NOT EXISTS idx_districts_bbox ON districts(min_lat, max_lat, min_lng, max_lng)')
    cursor.execute('CREATE INDEX IF NOT EXISTS idx_regencies_lat_lng ON regencies(lat, lng)')
    cursor.execute('CREATE INDEX IF NOT EXISTS idx_regencies_bbox ON regencies(min_lat, max_lat, min_lng, max_lng)')
    cursor.execute('CREATE INDEX IF NOT EXISTS idx_provinces_lat_lng ON provinces(lat, lng)')
    cursor.execute('CREATE INDEX IF NOT EXISTS idx_provinces_bbox ON provinces(min_lat, max_lat, min_lng, max_lng)')
    
    conn.commit()

def process_geojson(filepath):
    try:
        with open(filepath, 'r') as f:
            data = json.load(f)
            
        # Kadang berupa FeatureCollection, kadang langsung Feature
        if data.get('type') == 'FeatureCollection' and 'features' in data and len(data['features']) > 0:
            feature = data['features'][0]
        elif data.get('type') == 'Feature':
            feature = data
        else:
            print(f"Format GeoJSON tidak dikenali di {filepath}")
            return None
            
        geom = shape(feature['geometry'])
        
        # Simplifikasi polygon (Ramer-Douglas-Peucker)
        # tolerance dalam derajat. 0.0000009 setara dengan ~10cm di khatulistiwa
        simplified_geom = geom.simplify(SIMPLIFY_TOLERANCE, preserve_topology=True)
        
        # Hitung centroid
        centroid = simplified_geom.centroid
        lat, lng = centroid.y, centroid.x
        
        # Hitung bounding box (min_lng, min_lat, max_lng, max_lat)
        min_lng, min_lat, max_lng, max_lat = simplified_geom.bounds
        
        # Konversi ke WKB Binary
        wkb = wkb_dumps(simplified_geom)
        
        # Ekstrak properties
        filename = os.path.basename(filepath)
        code = filename.replace('.geojson', '')

        props = feature.get('properties', {})
        name = props.get('name', '')

        # Normalkan nama menjadi Title Case (misal: "JAWA BARAT" -> "Jawa Barat")
        if name:
            name = name.title()

        # Fallback jika id ada di properties
        if not code and 'code' in props:
             code = props['code']
        
        return {
            'id': code,
            'name': name,
            'lat': lat,
            'lng': lng,
            'min_lat': min_lat,
            'max_lat': max_lat,
            'min_lng': min_lng,
            'max_lng': max_lng,
            'boundaries': wkb
        }
    except Exception as e:
        print(f"Error processing {filepath}: {e}")
        return None

def process_level(conn, level_dir, table_name, has_parent=True):
    cursor = conn.cursor()
    files = glob.glob(os.path.join(DATA_DIR, level_dir, '*.geojson'))
    total_files = len(files)
    
    print(f"Processing {total_files} files in {level_dir}...")
    
    count = 0
    
    # Kumpulkan data dalam batch untuk insert lebih cepat
    batch = []
    
    for filepath in files:
        data = process_geojson(filepath)
        if data:
            if has_parent:
                # Parent ID adalah ID dikurangi bagian terakhir
                # Contoh: 32.12.02.2007 -> parent_id = 32.12.02
                parts = data['id'].split('.')
                parent_id = '.'.join(parts[:-1]) if len(parts) > 1 else None
                
                batch.append((data['id'], data['name'], parent_id, data['lat'], data['lng'], 
                      data['min_lat'], data['max_lat'], data['min_lng'], data['max_lng'], data['boundaries']))
            else:
                batch.append((data['id'], data['name'], data['lat'], data['lng'], 
                      data['min_lat'], data['max_lat'], data['min_lng'], data['max_lng'], data['boundaries']))
            
            count += 1
            if count % 1000 == 0:
                print(f"Processed {count}/{total_files}...")
                
                if has_parent:
                     cursor.executemany(f'''
                     INSERT OR REPLACE INTO {table_name} 
                     (id, name, parent_id, lat, lng, min_lat, max_lat, min_lng, max_lng, boundaries)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ''', batch)
                else:
                     cursor.executemany(f'''
                     INSERT OR REPLACE INTO {table_name} 
                     (id, name, lat, lng, min_lat, max_lat, min_lng, max_lng, boundaries)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ''', batch)
                     
                conn.commit()
                batch = []
                
    # Insert sisa batch
    if batch:
         if has_parent:
             cursor.executemany(f'''
             INSERT OR REPLACE INTO {table_name} 
             (id, name, parent_id, lat, lng, min_lat, max_lat, min_lng, max_lng, boundaries)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ''', batch)
         else:
             cursor.executemany(f'''
             INSERT OR REPLACE INTO {table_name} 
             (id, name, lat, lng, min_lat, max_lat, min_lng, max_lng, boundaries)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ''', batch)
         conn.commit()
         
    print(f"Finished processing {level_dir}. Total inserted: {count}")

def main():
    if os.path.exists(DB_FILE):
        print(f"Removing existing database {DB_FILE}...")
        os.remove(DB_FILE)
        
    print(f"Creating new database {DB_FILE}...")
    conn = sqlite3.connect(DB_FILE)
    
    try:
        init_db(conn)
        
        # Proses secara berurutan agar relasi parent terjamin
        process_level(conn, 'provinces', 'provinces', has_parent=False)
        process_level(conn, 'regencies', 'regencies', has_parent=True)
        process_level(conn, 'districts', 'districts', has_parent=True)
        process_level(conn, 'villages', 'villages', has_parent=True)
        
        print("Database creation completed successfully.")
    except Exception as e:
        print(f"An error occurred: {e}")
    finally:
        conn.close()

if __name__ == '__main__':
    main()
