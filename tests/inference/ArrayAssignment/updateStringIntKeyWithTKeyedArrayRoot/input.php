<?php
$string = "c";
$int = 5;

$b = [];

$b["root"][$string] = 5;
$b["root"][0] = 3;

$c = [];

$c["root"][0] = 3;
$c["root"][$string] = 5;

$d = [];

$d["root"][$int] = 3;
$d["root"]["a"] = 5;

$e = [];

$e["root"][$int] = 3;
$e["root"][$string] = 5;
