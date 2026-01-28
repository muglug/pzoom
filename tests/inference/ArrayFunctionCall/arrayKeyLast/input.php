<?php
/** @return array<string, int> */
function makeArray(): array { return ["one" => 1, "two" => 3]; }
$a = makeArray();
$b = array_key_last($a);
$c = null;
if ($b !== null) {
    $c = $a[$b];
}
