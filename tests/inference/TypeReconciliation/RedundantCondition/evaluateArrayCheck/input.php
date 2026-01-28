<?php
function array_check(): void {
    $data = ["f" => false];
    while (rand(0, 1) > 0 && !$data["f"]) {
        $data = ["f" => true];
    }
}