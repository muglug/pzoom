<?php
$headers = fgetcsv(fopen("test.txt", "r"));
foreach ($headers as $value) {
    echo (string) $value;
}
