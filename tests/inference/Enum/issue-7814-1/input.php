<?php
enum State
{
    case A;
    case B;
    case C;
}

/**
 * @param State::A|State::B $_
 */
function test(State $_): void {}

test(State::C);
                
