<?php
$a = ["a" => 3, "b" => 4];
ksort($a);
acceptsAShape($a);

/**
 * @param array{a:int,b:int} $a
 */
function acceptsAShape(array $a): void {}
