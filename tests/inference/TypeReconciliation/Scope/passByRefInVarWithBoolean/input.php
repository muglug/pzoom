<?php
$a = preg_match("/bad/", "badger", $matches) > 0;
if ($a) {
    echo $matches[1];
}