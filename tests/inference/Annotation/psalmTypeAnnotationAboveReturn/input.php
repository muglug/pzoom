<?php
/**
 * @psalm-type Person = array{name: string, age: int}
 */

/**
 * @psalm-return Person
 */
function getPerson_error(): array {
    $json = '{"name": "John", "age": 44}';
    /** @psalm-var Person */
    return json_decode($json, true);
}
