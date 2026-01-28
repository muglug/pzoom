<?php
if (!preg_match("/bad/", "badger", $matches)) {
    exit();
}
echo $matches[0];