<?php
$as = ["key"];
$bs = ["value"];

return array_map(fn ($a, $b) => [$a => $b], $as, $bs);
