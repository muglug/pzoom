<?php
$mysqli = new mysqli();

$a = $mysqli->escape_string($_GET["a"]);
$b = mysqli_escape_string($mysqli, $_GET["b"]);
$c = $mysqli->real_escape_string($_GET["c"]);
$d = mysqli_real_escape_string($mysqli, $_GET["d"]);

$mysqli->query("$a$b$c$d");
