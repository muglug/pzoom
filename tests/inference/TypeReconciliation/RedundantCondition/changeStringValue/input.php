<?php
$concat = "";
foreach (["x", "y"] as $v) {
    if ($concat != "") {
        $concat .= ", ";
    }
    $concat .= "($v)";
}