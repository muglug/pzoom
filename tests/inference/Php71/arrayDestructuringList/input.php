<?php
$data = [
    [1, "Tom"],
    [2, "Fred"],
];

// list() style
list($id1, $name1) = $data[0];

// [] style
[$id2, $name2] = $data[1];
