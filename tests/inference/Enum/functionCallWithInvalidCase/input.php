<?php
enum Status {
    case Open;
    case Closed;
}

/** @param Status::Open $status */
function foo(Status $status): void {}

foo(Status::Closed);
                
