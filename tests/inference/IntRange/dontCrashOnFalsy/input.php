<?php

function doAnalysis(): void
{
    /** @var int<3, max> $shuffle_count */
    $shuffle_count = 1;

    /** @var list<string> $file_paths */
    $file_paths = [];

    /** @var int<0, max> $count */
    $count = 1;
    /** @var int<0, max> $middle */
    $middle = 1;
    /** @var int<0, max> $remainder */
    $remainder = 1;


    for ($i = 0; $i < $shuffle_count; $i++) {
        for ($j = 0; $j < $middle; $j++) {
            if ($j * $shuffle_count + $i < $count) {
                echo $file_paths[$j * $shuffle_count + $i];
            }
        }

        if ($remainder) {
            echo $file_paths[$middle * $shuffle_count + $remainder - 1];
            $remainder--;
        }
    }
}
