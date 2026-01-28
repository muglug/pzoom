<?php
/**
 * @return non-empty-list<string>
 */
function foo(string $key): array {
    $elements = [$key];

    while (rand(0, 1)) {
        $elements[] = $key;
    }

    return $elements;
}
