<?php
/** @return no-return */
function other() {
    throw new Exception();
}

rand(0,1) ? die() : other();
