<?php
/**
 * @return key-of<list<int>|array{a: int, b: int}>
 */
function getKey(bool $asInt) {
    if ($asInt) {
        return 42;
    }
    return "a";
}
