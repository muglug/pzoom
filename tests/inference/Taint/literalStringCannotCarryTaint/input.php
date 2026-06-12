<?php
$file = $_GET["foo"];

if ($file !== "") {
    /**
     * @psalm-taint-escape input
     */
    $file = basename($file);
}

echo $file;
