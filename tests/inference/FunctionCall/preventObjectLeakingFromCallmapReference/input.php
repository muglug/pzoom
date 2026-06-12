<?php
function one(): void
{
    try {
        exec("", $output);
    } catch (Exception $e){
    }
}

function two(): array
{
    exec("", $lines);
    return $lines;
}
