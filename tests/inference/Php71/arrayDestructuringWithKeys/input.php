<?php
$data = [
    ["id" => 1, "name" => "Tom"],
    ["id" => 2, "name" => "Fred"],
];

// list() style
list("id" => $id1, "name" => $name1) = $data[0];

// [] style
["id" => $id2, "name" => $name2] = $data[1];
