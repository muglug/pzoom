<?php
function f(string $gap): void {
    $expected = [
        '->' => ['a', 'b'],
        '::' => ['c'],
    ];
    $labels = $expected[$gap];
    foreach ($labels as $l) { echo $l; }
}
