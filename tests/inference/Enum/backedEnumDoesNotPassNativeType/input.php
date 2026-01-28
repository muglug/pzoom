<?php
enum State: string
{
    case A = "A";
    case B = "B";
    case C = "C";
}
function f(string $state): void {}
f(State::A);
                
