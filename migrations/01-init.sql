-- Initial database setup for VibeDB testing

-- Create a users table for testing
CREATE TABLE IF NOT EXISTS users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    email VARCHAR(100) UNIQUE NOT NULL,
    role VARCHAR(50) DEFAULT 'user',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Insert sample data
INSERT INTO users (name, email, role) VALUES
    ('Alice Johnson', 'alice@example.com', 'admin'),
    ('Bob Smith', 'bob@example.com', 'user'),
    ('Charlie Brown', 'charlie@example.com', 'user'),
    ('Diana Prince', 'diana@example.com', 'moderator'),
    ('Eve Adams', 'eve@example.com', 'user');

-- Create additional test table with more data
CREATE TABLE IF NOT EXISTS products (
    id SERIAL PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    price DECIMAL(10,2),
    category VARCHAR(100),
    in_stock BOOLEAN DEFAULT true
);

-- Insert many products to test row limits
INSERT INTO products (name, price, category, in_stock) 
SELECT 
    'Product ' || i,
    (RANDOM() * 100)::DECIMAL(10,2),
    CASE (i % 5)
        WHEN 0 THEN 'Electronics'
        WHEN 1 THEN 'Books'
        WHEN 2 THEN 'Clothing'
        WHEN 3 THEN 'Home'
        ELSE 'Sports'
    END,
    RANDOM() > 0.1
FROM generate_series(1, 1000) i;

-- Create the honeytoken table (should trigger blocking)
CREATE TABLE IF NOT EXISTS _vibedb_canary (
    id SERIAL PRIMARY KEY,
    secret TEXT NOT NULL,
    accessed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Insert honeytoken data
INSERT INTO _vibedb_canary (secret) VALUES 
    ('This is a canary token - accessing this should be blocked!');

-- Create a view for testing
CREATE VIEW active_users AS
    SELECT id, name, email, role 
    FROM users 
    WHERE created_at > CURRENT_DATE - INTERVAL '30 days';

-- Add some indexes
CREATE INDEX idx_users_role ON users(role);
CREATE INDEX idx_products_category ON products(category);

-- Grant permissions (for completeness)
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO postgres;