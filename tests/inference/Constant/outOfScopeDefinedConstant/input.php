<?php
namespace {
    define("A\B", 0);
}
namespace C {
    echo A\B;
}
