document.addEventListener('DOMContentLoaded', () => {
    // Tab switching logic
    const tabBtns = document.querySelectorAll('.install-tab');
    const tabContents = document.querySelectorAll('.install-cmd');
    const viewScriptLink = document.getElementById('view-script-btn');

    tabBtns.forEach(btn => {
        btn.addEventListener('click', () => {
            // Remove active class from all buttons and contents
            tabBtns.forEach(b => b.classList.remove('active'));
            tabContents.forEach(c => c.classList.add('install-cmd-hidden'));

            // Add active class to clicked button and target content
            btn.classList.add('active');
            const targetId = btn.getAttribute('data-target');
            document.getElementById(targetId).classList.remove('install-cmd-hidden');
            
            // Update "View full script" link
            if (viewScriptLink) {
                const isWindows = targetId === 'cmd-windows';
                viewScriptLink.href = isWindows 
                    ? 'https://raw.githubusercontent.com/youssefsz/reposweep/main/install.ps1'
                    : 'https://raw.githubusercontent.com/youssefsz/reposweep/main/install.sh';
            }
        });
    });

    // Copy to clipboard logic
    const copyBtns = document.querySelectorAll('.copy-btn');
    copyBtns.forEach(btn => {
        btn.addEventListener('click', () => {
            const targetId = btn.getAttribute('data-clipboard');
            const codeBlock = document.getElementById(targetId).innerText;
            
            navigator.clipboard.writeText(codeBlock).then(() => {
                const originalText = btn.innerText;
                btn.innerText = 'Copied';
                btn.style.color = '#10b981'; // Success green briefly
                btn.style.borderColor = '#10b981';

                setTimeout(() => {
                    btn.innerText = originalText;
                    btn.style.color = '';
                    btn.style.borderColor = '';
                }, 2000);
            }).catch(err => {
                console.error('Failed to copy: ', err);
            });
        });
    });
});