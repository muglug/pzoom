<?php
if (preg_match("/bad/", "badger", $matches)) {
    echo $matches[0];
}