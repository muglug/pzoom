<?php
function processParams(array $params) : array {
    if (isset($params["foo"])) {
        return $params;
    }

    return [];
}

$params = processParams($_GET);

echo $params["foo"];
