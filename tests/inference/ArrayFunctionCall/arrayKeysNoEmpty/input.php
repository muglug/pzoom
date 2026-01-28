<?php
function expect_string(string $x): void {
    echo $x;
}

function test(): void {
    foreach (array_keys([]) as $key) {
        expect_string($key);
    }
}
