#!/bin/bash

PORT=$1
FRAMEWORK=$2

echo "==========================================="
echo " Starting Benchmark for $FRAMEWORK on Port $PORT"
echo "==========================================="

run_test() {
    ENDPOINT=$1
    METHOD=$2
    DATA_FILE=$3
    echo -e "\n---> Testing: $ENDPOINT ($METHOD)"
    
    for CONCURRENCY in 50 250 500 1000; do
        echo "Concurrency: $CONCURRENCY"
        if [ "$METHOD" == "POST" ]; then
            oha -c $CONCURRENCY -z 10s -m POST -T application/json -d @$DATA_FILE --no-tui http://127.0.0.1:$PORT$ENDPOINT | grep -E "Requests/sec|99.00%"
        else
            oha -c $CONCURRENCY -z 10s --no-tui http://127.0.0.1:$PORT$ENDPOINT | grep -E "Requests/sec|99.00%"
        fi
    done
}

run_test "/large-json" "GET" ""
run_test "/prime" "GET" ""
run_test "/parallel" "GET" ""
run_test "/users/123/orders/456/items/789" "GET" ""
run_test "/bulk" "POST" "large.json"
run_test "/validate" "POST" "validate.json"
run_test "/mixed" "POST" "mixed.json"