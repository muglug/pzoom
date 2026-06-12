<?php
enum Status: int {
    case Open = 1;
    case Closed = 2;
}

$_z = Status::from(2);
