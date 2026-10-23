# A-Fan

> **PROJETO EXPERIMENTAL — CONTROLE FÍSICO DE FAN NÃO TESTADO NEM VALIDADO.**
>
> Desenvolvido especificamente para o **Acer Aspire A515-45 com AMD Ryzen 7 5700U, BIOS V1.58 e Windows 11 x64**. Há testes de software e leituras de sensores, mas a redução real da rotação da ventoinha não foi validada neste hardware.

Aplicativo Windows x64 nativo em Rust/Win32 para o Acer Aspire A515-45. Abre oculto na bandeja; fechar ou minimizar a janela mantém o monitoramento. O menu **Sair** encerra após restaurar o controle Acer se o aplicativo estiver atuando.

## Situação neste notebook

Diagnóstico em 23/10/2026: Aspire A515-45, Ryzen 7 5700U, BIOS V1.58. Temperatura CPU e RPM funcionam via WMI. Os getters de modo e porcentagem da ventoinha retornam status **1**, indicando comando indisponível. Portanto, **o A-Fan não altera a fan neste notebook**: monitoramento e prévias de presets funcionam; aplicar presets A-Fan e calibrar permanecem bloqueados. Nenhum setter foi executado na validação.

O aplicativo não modifica plano de energia, boost, limites CPU/GPU, perfis Acer nem BIOS. Não inclui escrita bruta no EC, drivers de acesso arbitrário a hardware, rede, telemetria externa ou atualização automática.

## Instalar

Este repositório publica o código-fonte; binários, diagnósticos e relatórios da máquina de desenvolvimento ficam fora do Git. Para gerar o pacote, é necessário Windows x64 com Rust estável e ferramentas de compilação C++ e Windows SDK instalados. Na raiz do projeto, execute no PowerShell:

```powershell
.\scripts\build.ps1
```

Depois, na pasta gerada `artifacts/release`, execute `install.ps1` com PowerShell. A instalação solicita administrador, copia o app para `%ProgramFiles%\A-Fan`, cria atalhos e registra a tarefa `A-Fan` para o login. Para remover, execute `%ProgramFiles%\A-Fan\uninstall.ps1`.

## Presets fixos

Não há ajuste manual de rotação, temperatura nem pontos da curva. A seleção exibe uma prévia e uma descrição; **Aplicar perfil** exige sensores, controle independente e processo de recuperação válido.

| Preset | 40°C | 55°C | 65°C | 75°C | 80°C ou mais |
| --- | ---: | ---: | ---: | ---: | ---: |
| Discreto | 40% | 40% | 55% | 85% | 100% |
| Auto | 40% | 50% | 65% | 90% | 100% |
| Resfriar | 50% | 65% | 80% | 100% | 100% |
| Carga intensa | 60% | 80% | 100% | 100% | 100% |
| Ventilação máxima | 100% | 100% | 100% | 100% | 100% |

Os percentuais representam comandos de ventilação, não RPM nem porcentagens de desempenho. O mínimo efetivo é **40% ou o mínimo calibrado, se maior**. Não há parada intencional da fan.

**Automático Acer** devolve a administração ao firmware, cuja curva não é conhecida. As proteções de curva do A-Fan só atuam quando um preset A-Fan tem controle físico validado. Sem suporte, o aplicativo monitora e avisa; não consegue impor rotação nem temperatura.

## Diagnóstico e desenvolvimento

- `a-fan.exe --diagnose caminho.json`: diagnóstico nativo somente de leitura; execute elevado para acessar a WMI Acer.
- `scripts/diagnose.ps1`: diagnóstico independente em PowerShell, também somente de leitura.
- `a-fan.exe --show`: abrir a janela durante desenvolvimento; a abertura normal permanece oculta.
- `a-fan.exe --exit`: solicitar saída segura de uma instância da mesma sessão.
- `scripts/build.ps1`: formatação, testes, Clippy, release e pacote com SHA-256.

## Fontes e evidências

- [Linux acer-wmi oficial](https://kernel.googlesource.com/pub/scm/linux/kernel/git/torvalds/linux/+/refs/heads/master/drivers/platform/x86/acer-wmi.c): contrato WMI, status, sensores e controle independente.
- [Microsoft: números WMI](https://learn.microsoft.com/en-us/windows/win32/wmisdk/numbers): uint64 em VARIANT/BSTR e uint32 em VT_I4.
- [AMD Ryzen 7 5700U](https://www.amd.com/en/support/downloads/drivers.html/processors/ryzen/ryzen-5000-series/amd-ryzen-7-5700u.html): Tjmax de 105°C.
- `scripts/diagnose.ps1` gera `artifacts/diagnostics/hardware.json` localmente. Esse arquivo e os demais diagnósticos não são versionados.

Controles físicos de fan e recuperação em falha não puderam ser ensaiados neste hardware, porque o gate de suporte falhou. Testes de software ou prévias de curva não constituem validação física.
