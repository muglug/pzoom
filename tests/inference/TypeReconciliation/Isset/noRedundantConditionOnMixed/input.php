<?php
function testarray(array $data): void {
    foreach ($data as $item) {
        if (isset($item["a"]) && isset($item["b"]["c"])) {
            echo "Found\n";
        }
    }
}