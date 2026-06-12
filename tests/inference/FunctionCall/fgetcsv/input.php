<?php
$headers = fgetcsv(fopen("test.txt", "r"));
if (empty($headers)) {
    throw new Exception("invalid headers");
}
print_r(array_map("strval", $headers));
