<?php
$a = "a";
$b = false;
$doesNotMatter = ["a" => ["id" => 1]];
$reference = &$doesNotMatter[$a];
/** @psalm-suppress TypeDoesNotContainType */
$result = ($a === "not-a" && ($b || false));
                
