<?php
function consume(string $value): void {
    echo $value;
}

/** @var object{value?: string} $data */
$data = json_decode("{}", false);
consume($data->value ?? "");
