<?php
const cons1 = 0;

class Clazz {
    const cons2 = cons1;
    const cons3 = 0;
}

echo cons1;
echo Clazz::cons2;
echo Clazz::cons3;
