<?php
$data = [
    [1, "Tom"],
    [2, "Fred"],
];

// [] style
foreach ($data as [$id, $name]) {
    echo $id;
    echo $name;
}
