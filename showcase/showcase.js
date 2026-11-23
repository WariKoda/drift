const switcher = document.querySelector('.switcher');
const tabs = Array.from(switcher.querySelectorAll('button'));
const panels = tabs.map(tab => document.getElementById(tab.getAttribute('aria-controls')));
const video = document.querySelector('video');

function selectView(index) {
  tabs.forEach((tab, position) => {
    const active = position === index;
    tab.setAttribute('aria-selected', String(active));
    tab.tabIndex = active ? 0 : -1;
    panels[position].hidden = !active;
  });
  if (panels[index].id !== 'demo') video.pause();
}

function selectLinkedView() {
  const index = panels.findIndex(panel => '#' + panel.id === location.hash);
  if (index !== -1) {
    selectView(index);
    panels[index].scrollIntoView();
  }
}

switcher.setAttribute('role', 'tablist');
tabs.forEach((tab, index) => {
  tab.setAttribute('role', 'tab');
  panels[index].setAttribute('role', 'tabpanel');
  panels[index].setAttribute('aria-labelledby', tab.id);
  panels[index].tabIndex = 0;

  tab.addEventListener('click', () => selectView(index));
  tab.addEventListener('keydown', event => {
    let next;
    switch (event.key) {
      case 'ArrowRight': next = (index + 1) % tabs.length; break;
      case 'ArrowLeft': next = (index + tabs.length - 1) % tabs.length; break;
      case 'Home': next = 0; break;
      case 'End': next = tabs.length - 1; break;
      default: return;
    }
    event.preventDefault();
    tabs[next].focus();
    selectView(next);
  });
});
switcher.hidden = false;
selectView(0);
selectLinkedView();
window.addEventListener('hashchange', selectLinkedView);
document.querySelector('.watch').addEventListener('click', () => {
  selectView(panels.findIndex(panel => panel.id === 'demo'));
});

const german = document.documentElement.lang === 'de';
const copyButton = document.getElementById('copy');
const copyStatus = document.getElementById('copy-status');
if (navigator.clipboard?.writeText) {
  copyButton.hidden = false;
  copyButton.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(document.getElementById('install-command').textContent);
      copyStatus.textContent = german ? 'Befehl kopiert.' : 'Command copied.';
    } catch {
      copyStatus.textContent = german ? 'Bitte markiere und kopiere den Befehl direkt.' : 'Select the command and copy it manually.';
    }
  });
}
