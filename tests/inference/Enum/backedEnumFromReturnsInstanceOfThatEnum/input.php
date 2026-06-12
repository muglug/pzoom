<?php
enum Status: int {
    case Open = 1;
    case Closed = 2;
}

function f(): Status {
    return Status::from(1);
}
