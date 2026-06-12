<?php
/** @psalm-suppress MixedAssignment */
$array = $_GET["foo"] ?? [];

$array[$a] = "b";
