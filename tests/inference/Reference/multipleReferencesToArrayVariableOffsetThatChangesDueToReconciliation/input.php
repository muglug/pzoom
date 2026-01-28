<?php
$a = "a";
$b = false;
$doesNotMatter = ["a" => ["id" => 1]];
$reference1 = &$doesNotMatter[$a];
$reference2 = &$doesNotMatter[$a];
/** @psalm-suppress TypeDoesNotContainType */
$result = ($a === "not-a" && ($b || false));
$reference1["id"] = 2;
                
