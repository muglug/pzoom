<?php
interface One {}
interface Two {}

/**
 * @param One|Two $value
 */
function isOne($value): void {
    if ($value instanceof One) {
        if ($value instanceof One) {}
    }
}
