Движок SSBZapret основан на winws из проекта Zapret (bol-van/zapret).
Сюда нужно положить файлы из дистрибутива Zapret (папка binaries\win64):

  winws.exe          — сам движок обхода DPI
  cygwin1.dll        — ОБЯЗАТЕЛЬНО! winws собран на Cygwin и без неё не запустится
  WinDivert.dll      — пользовательская библиотека
  WinDivert64.sys    — драйвер ядра
  любые другие cyg*.dll и *.bin — если они есть рядом с winws.exe

!!! САМОЕ ПРОСТОЕ: скопируйте ВСЁ содержимое папки binaries\win64
из Zapret целиком в эту папку resources/. Лишнее не помешает,
а вот нехватка cygwin1.dll даёт ошибку "система не обнаружила cygwin1.dll".

Где взять: Releases на github.com/bol-van/zapret -> ассет zapret-vXX.zip
(НЕ "Source code") -> распаковать -> папка binaries\win64.

Эти файлы подключаются на этапе сборки (tauri.conf.json -> bundle.resources,
glob "resources/*") и попадают в установщик.
