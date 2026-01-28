<?php
$name = rand(0, 1) ? "hello" : null;
if ($name !== null || ($name = rand(0, 1) ? "hello" : null) !== null) {}
