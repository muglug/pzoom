<?php
function foo(): \Generator {
    /** @var array<string, mixed> $_SERVER */
    foreach (array_filter(array_keys($_SERVER), function (string $key) : bool {
        return strpos($key, "BAR") === 0;
    }) as $envVar) {
        yield $envVar => [getenv($envVar)];
    }
}
