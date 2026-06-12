<?php
function getName(array $data) : string {
    return $data["name"] ?? "unknown";
}

$name = getName($_GET);

echo $name;
