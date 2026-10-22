const express = require('express');
const app = express();
app.use(express.json());

const USERS_RAW = Array.from({ length: 50 }, (_, i) => ({
    id: i,
    name: `User_${i}`,
    email: `user${i}@example.com`,
    role: i % 2 === 0 ? 'admin' : 'user',
    secret: 'hidden_token'
}));

// 1. Sync JSON
app.get('/json', (req, res) => {
    res.json({ status: 'ok', message: 'hello' });
});

// 2. Async JSON
app.get('/async_json', async (req, res) => {
    res.json({ status: 'ok', message: 'async hello' });
});

// 3. POST /items (manual validation matching Item schema)
app.post('/items', (req, res) => {
    const { name, price, quantity } = req.body || {};
    if (typeof name !== 'string' || typeof price !== 'number' || typeof quantity !== 'number') {
        return res.status(422).json({ error: 'Unprocessable Entity' });
    }
    res.json({ name, total: price * quantity });
});

// 4. GET /users (50 items with secret filtered out)
app.get('/users', (req, res) => {
    const filtered = USERS_RAW.map(({ id, name, email, role }) => ({ id, name, email, role }));
    res.json(filtered);
});

const PORT = 8094;
app.listen(PORT, '127.0.0.1', () => {
    console.log(`Express server running on http://127.0.0.1:${PORT}`);
});
