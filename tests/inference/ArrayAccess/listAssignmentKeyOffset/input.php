<?php
$a = [];
list($a["foo"]) = explode("+", "a+b");
echo $a["foo"];
