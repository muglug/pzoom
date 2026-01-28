<?php
$data = [
    ["id" => 1, "name" => "Tom"],
    ["id" => 2, "name" => "Fred"],
];

// [] style
foreach ($data as ["id" => $id, "name" => $name]) {
    $last_id = $id;
    $last_name = $name;
}
