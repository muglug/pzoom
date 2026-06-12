<?php
enum Status: int {
    case Open = 1;
    case Closed = 2;
    case Busted = 3;
}

$_z = Status::from(rand(1, 2));
