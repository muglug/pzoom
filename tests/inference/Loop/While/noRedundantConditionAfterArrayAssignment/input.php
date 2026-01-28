<?php
$data = ["a" => false];
while (!$data["a"]) {
    if (rand() % 2 > 0) {
        $data = ["a" => true];
    }
}
