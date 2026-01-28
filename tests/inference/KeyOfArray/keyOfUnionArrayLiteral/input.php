<?php
/**
 * @return key-of<array<int, string>|array<string, string>>
 */
function getKey(bool $asString) {
    if ($asString) {
        return "42";
    }
    return 42;
}
