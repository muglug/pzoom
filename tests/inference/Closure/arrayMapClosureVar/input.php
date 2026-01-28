<?php
$mirror = function(int $i) : int { return $i; };
$a = array_map($mirror, [1, 2, 3]);
