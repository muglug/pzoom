<?php
$a = (bool)rand(0, 1);
if ($a && preg_match("/bad/", "badger", $matches)) {
    echo $matches[0];
}