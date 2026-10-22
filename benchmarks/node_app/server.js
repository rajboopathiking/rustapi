const fastify = require('fastify')({ logger: false });

const USERS_RAW = Array.from({ length: 50 }, (_, i) => ({
    id: i,
    name: `User_${i}`,
    email: `user${i}@example.com`,
    role: i % 2 === 0 ? 'admin' : 'user',
    secret: 'hidden_token'
}));

// 1. Sync/Direct JSON
fastify.get('/json', (req, reply) => {
    reply.send({ status: 'ok', message: 'hello' });
});

// 2. Async JSON
fastify.get('/async_json', async (req, reply) => {
    return { status: 'ok', message: 'async hello' };
});

// 3. Validated POST (Ajv JSON Schema)
fastify.post('/items', {
    schema: {
        body: {
            type: 'object',
            required: ['name', 'price', 'quantity'],
            properties: {
                name: { type: 'string' },
                price: { type: 'number' },
                quantity: { type: 'integer' }
            }
        }
    }
}, (req, reply) => {
    const { name, price, quantity } = req.body;
    reply.send({ name, total: price * quantity });
});

// 4. Response Model / Filtered Schema (fast-json-stringify filters out 'secret')
fastify.get('/users', {
    schema: {
        response: {
            200: {
                type: 'array',
                items: {
                    type: 'object',
                    properties: {
                        id: { type: 'integer' },
                        name: { type: 'string' },
                        email: { type: 'string' },
                        role: { type: 'string' }
                    }
                }
            }
        }
    }
}, (req, reply) => {
    reply.send(USERS_RAW);
});

fastify.listen({ port: 8093, host: '127.0.0.1' }, (err, address) => {
    if (err) {
        console.error(err);
        process.exit(1);
    }
    console.log(`Fastify listening on ${address}`);
});
