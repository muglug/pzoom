<?php
if (rand(0, 4) > 2) {
    $arr = [5 => [3 => "hello"]];
}

if (isset($arr[$a = 5][$b = 3])) {

}

echo $a;
echo $b;