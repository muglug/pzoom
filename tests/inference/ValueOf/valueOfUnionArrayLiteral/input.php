<?php
/**
 * @return value-of<array<array-key, int>|array<string, float>>
 */
function getValue(bool $asFloat) {
    if ($asFloat) {
        return 42.0;
    }
    return 42;
}
