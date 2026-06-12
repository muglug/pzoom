<?php
$name = $_GET["name"] ?? "unknown";
$id = (int) $_GET["id"];

$data = ["name" => $name, "id" => $id];

echo "<h1>" . htmlentities($data["name"], \ENT_QUOTES) . "</h1>";
echo "<p>" . $data["id"] . "</p>";
