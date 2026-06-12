<?php
$name = $_GET["name"] ?? "unknown";

$data = ["name" => $name];

echo "<h1>" . $data["name"] . "</h1>";
