<?php
$a = rand(0,1) ? new DateTime() : null;

if (($a !== null && $a->format("Y") === "2020") == true) {
    $a->format("d-m-Y");
}