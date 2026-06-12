<?php
function foo(array $arr): \Generator {
    /** @var array<string, mixed> $arr */
    foreach (array_filter(array_keys($arr), function (string $key) : bool {
        return strpos($key, "BAR") === 0;
    }) as $envVar) {
        yield $envVar => [getenv($envVar)];
    }
}
