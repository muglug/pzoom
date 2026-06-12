<?php
function f(array $row): void {
    $name = $row['name'] ?? '';
    if (!is_string($name)) {
        $name = 'unknown';
    }
    echo $name;
}
