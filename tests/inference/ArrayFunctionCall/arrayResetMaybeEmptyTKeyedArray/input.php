<?php
/** @return array{foo?: int} */
function makeArray(): array { return []; }
$a = makeArray();
$b = reset($a);
