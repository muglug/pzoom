<?php
/** @return array<string, int> */
function makeArray(): array { return ["one" => 1, "two" => 3]; }
$a = makeArray();
$b = reset($a);
