<?php
class Graph4 {}

/**
 * @param array{taint_data: Graph4|null} $pool_data
 */
function merge5(array $pool_data): void {
    if ($pool_data['taint_data']) {
        $g = $pool_data['taint_data'];
        echo get_class($g);
    }
}

/** @param Graph4|null $td */
function merge6($td): void {
    $pool_data = ['taint_data' => $td];
    if ($pool_data['taint_data']) {
        $g = $pool_data['taint_data'];
        echo get_class($g);
    }
}
