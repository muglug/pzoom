<?php
enum Status: int {
    case Open = 1;
    case Closed = 2;
}

function f(): Status {
    return Status::tryFrom(rand(1, 10)) ?? Status::Open;
}
