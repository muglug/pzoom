<?php
/**
 * @return array{int, int}
 */
function size(): array {
    return [10, 20];
}

[$width, $height, $depth] = size();
