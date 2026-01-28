<?php
function takesString(string $s) : void {}
$a = fopen("php://memory", "r");
takesString($a);
