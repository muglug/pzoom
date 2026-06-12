<?php
function getArray() : array { return []; }
$params = array(
    "a" => 1,
    "b" => [
        "c" => "a",
    ]
);

if (rand(0, 1)) {
    $params = getArray();
}

echo $params["b"]["c"];
