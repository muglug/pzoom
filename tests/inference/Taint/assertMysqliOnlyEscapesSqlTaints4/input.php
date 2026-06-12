<?php
$mysqli = new mysqli();
echo mysqli_real_escape_string($mysqli, $_GET["a"]);
