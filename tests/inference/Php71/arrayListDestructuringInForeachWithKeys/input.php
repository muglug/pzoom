<?php
$data = [
    ["id" => 1, "name" => "Tom"],
    ["id" => 2, "name" => "Fred"],
];

// list() style
foreach ($data as list("id" => $id, "name" => $name)) {
    $last_id = $id;
    $last_name = $name;
}
