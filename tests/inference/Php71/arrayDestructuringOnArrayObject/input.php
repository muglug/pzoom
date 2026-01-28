<?php
$var = new ArrayObject([0 => "first", "dos" => "second"]);
[0 => $first, "dos" => $second] = $var;
echo $first;
echo $second;
