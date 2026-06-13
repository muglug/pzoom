<?php
function a(array $in): void
{
    $r = [];
    foreach ($in as $entry) {
        if (!empty($entry["a"])) {
            $r[] = [];
        }
        if (empty($entry["a"])) {
            $r[] = [];
        }
    }
}

function b(array $in): void
{
    $i = 0;
    foreach ($in as $entry) {
        if (!empty($entry["a"])) {
            $i--;
        }
        if (empty($entry["a"])) {
            $i++;
        }
    }
}

function c(array $in): void
{
    foreach ($in as $entry) {
        if (!empty($entry["a"])) {}
    }
    foreach ($in as $entry) {
        if (empty($entry["a"])) {}
    }
}
