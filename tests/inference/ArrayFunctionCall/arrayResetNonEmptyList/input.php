<?php
/** @return non-empty-list<int> */
function makeArray(): array { return [1, 3]; }
$a = makeArray();
$b = reset($a);
